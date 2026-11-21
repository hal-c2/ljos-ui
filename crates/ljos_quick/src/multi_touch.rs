//! `MultiPointTouchArea`: an item that follows every finger on it, each
//! as a `TouchPoint`, and the mouse as one more.
//!
//! The scene hands it the fingers that come down on it first
//! ([`crate::QuickScene::touch_event`]) and, while `mouseEnabled`, the
//! left button as a pointer item. What it tells, all measured on qml6:
//!
//! - an event is taken only while the fingers in it, those lifted in it
//!   too, are from `minimumTouchPoints` to `maximumTouchPoints`; one out
//!   of that range is dropped whole, so a point lifted in it stays
//!   pressed;
//! - a finger gets the first declared `touchPoints` entry that is free,
//!   or else a point the area makes, which it keeps for the next finger;
//! - in a taken event every point the area holds that was not lifted is
//!   in `updated`, still ones too, and the signals go `gestureStarted`,
//!   `released`, `canceled`, `updated`, `pressed`, then `touchUpdated`
//!   with the points still down;
//! - `gestureStarted` is told on each event while a point still down is
//!   further than the drag threshold from where it came down along an
//!   axis, until a handler calls `gesture.grab()`;
//! - `previousX`/`previousY` are where the finger was before it last
//!   moved, in a taken event or not, and a still point's values are left
//!   alone; a point whose finger came down in an event the area did not
//!   take has no `startX`/`startY` (0, 0).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, MetaRef, ObjectId, QmlList, QmlObject, QmlType, QtObject, Rect,
    Size, Value, Vector2d, qml_impl,
};
use ljos_render::kurbo;
use ljos_runtime::Time;

use crate::handlers::{emit, flag, num, put};
use crate::input::{MouseButtons, PointerEvent, PointerId};
use crate::item::{Item, is_a};
use crate::scene::{DRAG_THRESHOLD, local_point};

/// `TouchPoint` (QtQuick): one finger, or the mouse, on a
/// `MultiPointTouchArea`. Positions are in the area's space but `sceneX`
/// and `sceneY`. Read-only, as in Qt: only the area writes them.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "TouchPoint", extends = QtObject)]
pub struct TouchPoint {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly)]
    pub point_id: i32,
    /// A `pointingDeviceUniqueId`: no device here tells one, so its
    /// `numericId` is -1, as Qt's is for a finger.
    #[qml(notify, readonly)]
    pub unique_id: Value,
    #[qml(notify, readonly)]
    pub pressed: bool,
    #[qml(notify, readonly)]
    pub x: f64,
    #[qml(notify, readonly)]
    pub y: f64,
    #[qml(notify, readonly)]
    pub ellipse_diameters: Size,
    #[qml(notify, readonly)]
    pub pressure: f64,
    #[qml(notify, readonly)]
    pub rotation: f64,
    /// Pixels a second in the scene, from the point's last move.
    #[qml(notify, readonly)]
    pub velocity: Vector2d,
    #[qml(notify, readonly)]
    pub area: Rect,
    #[qml(notify, readonly)]
    pub start_x: f64,
    #[qml(notify, readonly)]
    pub start_y: f64,
    #[qml(notify, readonly)]
    pub previous_x: f64,
    #[qml(notify, readonly)]
    pub previous_y: f64,
    #[qml(notify, readonly)]
    pub scene_x: f64,
    #[qml(notify, readonly)]
    pub scene_y: f64,
}

fn no_unique_id() -> Value {
    Value::Map(std::sync::Arc::new(vec![(
        Atom::from("numericId"),
        Value::Int(-1),
    )]))
}

impl Default for TouchPoint {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            point_id: 0,
            unique_id: no_unique_id(),
            pressed: false,
            x: 0.0,
            y: 0.0,
            // qml6's, until a finger is on the point.
            ellipse_diameters: Size {
                width: -1.0,
                height: -1.0,
            },
            pressure: 0.0,
            rotation: 0.0,
            velocity: Vector2d::default(),
            area: Rect::default(),
            start_x: 0.0,
            start_y: 0.0,
            previous_x: 0.0,
            previous_y: 0.0,
            scene_x: 0.0,
            scene_y: 0.0,
        }
    }
}

/// `GestureEvent` (QtQuick): what `gestureStarted` is given.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "GestureEvent", extends = QtObject, uncreatable)]
pub struct GestureEvent {
    #[qml(base)]
    pub object: QtObject,
    /// The points still down.
    #[qml(list, readonly, holds = "TouchPoint")]
    pub touch_points: QmlList,
    #[qml(readonly)]
    pub drag_threshold: f64,
    #[qml(skip)]
    grabbed: Rc<Cell<bool>>,
}

impl Default for GestureEvent {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            touch_points: QmlList::default(),
            drag_threshold: DRAG_THRESHOLD,
            grabbed: Rc::default(),
        }
    }
}

#[qml_impl]
impl GestureEvent {
    /// Keeps the points for the area: it stops telling `gestureStarted`
    /// until every point is up.
    #[qml(invokable)]
    fn grab(cx: &Cx) {
        let _ = cx
            .engine
            .with(cx.this, |g: &GestureEvent| g.grabbed.set(true));
    }
}

/// `MultiPointTouchArea` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "MultiPointTouchArea", extends = Item)]
pub struct MultiPointTouchArea {
    #[qml(base)]
    pub item: Item,
    /// The points an author declared, given to fingers in this order.
    #[qml(list, append = point_append, holds = "TouchPoint")]
    pub touch_points: QmlList,
    #[qml(notify)]
    pub minimum_touch_points: i32,
    #[qml(notify)]
    pub maximum_touch_points: i32,
    /// The left button is one more point; while false the mouse goes
    /// through to what is under the area.
    #[qml(notify)]
    pub mouse_enabled: bool,
    #[qml(skip)]
    state: Rc<AreaState>,
}

impl Default for MultiPointTouchArea {
    fn default() -> Self {
        Self {
            item: Item::default(),
            touch_points: QmlList::new(),
            minimum_touch_points: 0,
            maximum_touch_points: i32::MAX,
            mouse_enabled: true,
            state: Rc::default(),
        }
    }
}

#[qml_impl]
impl MultiPointTouchArea {
    #[qml(signal)]
    fn pressed(touch_points: Value);
    #[qml(signal)]
    fn updated(touch_points: Value);
    #[qml(signal)]
    fn released(touch_points: Value);
    #[qml(signal)]
    fn canceled(touch_points: Value);
    #[qml(signal, name = "gestureStarted")]
    fn gesture_started(gesture: ObjectId);
    #[qml(signal, name = "touchUpdated")]
    fn touch_updated(touch_points: Value);
}

fn point_append(cx: &Cx, point: ObjectId) -> Result<(), EngineError> {
    if !is_a::<TouchPoint>(cx.engine, point) {
        return Err(EngineError::WrongType {
            ty: "MultiPointTouchArea.touchPoints".into(),
            expected: "a TouchPoint".into(),
        });
    }
    cx.engine
        .list_push(cx.this, MultiPointTouchArea::TOUCH_POINTS.index, point)
}

/// Who a finger on an area is: the mouse and the fingers number their
/// pointers apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Who {
    Mouse,
    Finger(PointerId),
}

/// What happened to one of an area's pointers. `pos` is in the scene.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Change {
    pub who: Who,
    pub phase: crate::touch::TouchPhase,
    pub pos: kurbo::Point,
    pub time: Time,
}

#[derive(Debug)]
struct Finger {
    who: Who,
    /// Where it is now, in the scene, whether the area took it or not.
    pos: kurbo::Point,
    /// Where it was before it last moved, taken or not.
    prev: kurbo::Point,
    /// Where it came down, which the drag threshold is measured from
    /// even when the point has no start.
    down: kurbo::Point,
    /// Whether it came down in this event.
    began: bool,
    /// The point it has once a taken event gave it one.
    point: Option<ObjectId>,
    moved: bool,
    ended: Option<crate::touch::TouchPhase>,
    /// When its point was last told.
    time: Time,
    /// When it last came in an event, taken or not: a point catching up
    /// on a move the area did not take moved at that move's time.
    seen: Time,
}

#[derive(Debug, Default)]
struct AreaState {
    /// In the order they came down.
    fingers: RefCell<Vec<Finger>>,
    /// Points the area made, free for the next finger.
    spare: RefCell<Vec<ObjectId>>,
    /// The points the area made, free or not.
    made: RefCell<Vec<ObjectId>>,
    grabbed: Cell<bool>,
}

fn state(engine: &Engine, area: ObjectId) -> Option<Rc<AreaState>> {
    engine
        .with(area, |a: &MultiPointTouchArea| a.state.clone())
        .ok()
}

/// Whether `obj` is an area that takes the fingers that come down on it.
pub(crate) fn takes_touch(engine: &Engine, obj: ObjectId) -> bool {
    is_a::<MultiPointTouchArea>(engine, obj)
        && flag(engine, obj, Item::ENABLED.index)
        && flag(engine, obj, Item::VISIBLE.index)
}

fn list(points: &[ObjectId]) -> Value {
    Value::List(std::sync::Arc::new(
        points.iter().map(|&p| Value::Object(p)).collect(),
    ))
}

fn declared(engine: &Engine, area: ObjectId) -> Vec<ObjectId> {
    match engine.untrack(|| engine.read(area, MultiPointTouchArea::TOUCH_POINTS.index)) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|p| engine.is_alive(*p))
            .collect(),
        _ => Vec::new(),
    }
}

/// A point for a new finger: the first declared one no finger has, or
/// one the area keeps for this.
fn free_point(engine: &Engine, area: ObjectId, st: &AreaState) -> Option<ObjectId> {
    let taken: Vec<ObjectId> = st.fingers.borrow().iter().filter_map(|f| f.point).collect();
    if let Some(p) = declared(engine, area)
        .into_iter()
        .find(|p| !taken.contains(p))
    {
        return Some(p);
    }
    st.spare.borrow_mut().retain(|p| engine.is_alive(*p));
    if let Some(p) = st.spare.borrow_mut().pop() {
        return Some(p);
    }
    let p = engine
        .create_native(TouchPoint::default(), Some(area))
        .ok()?;
    st.made.borrow_mut().push(p);
    Some(p)
}

fn set(engine: &Engine, p: ObjectId, prop: ljos_engine::PropIndex, v: Value) {
    put(engine, p, prop, v);
}

fn real(engine: &Engine, p: ObjectId, prop: ljos_engine::PropIndex, v: f64) {
    set(engine, p, prop, Value::Real(v));
}

/// Where the point is now: its position in the area and the scene, and
/// the finger's area of contact, which is a point here.
fn place(engine: &Engine, area: ObjectId, p: ObjectId, scene: kurbo::Point) {
    let l = local_point(engine, area, scene);
    real(engine, p, TouchPoint::X.index, l.x);
    real(engine, p, TouchPoint::Y.index, l.y);
    real(engine, p, TouchPoint::SCENE_X.index, scene.x);
    real(engine, p, TouchPoint::SCENE_Y.index, scene.y);
    set(
        engine,
        p,
        TouchPoint::AREA.index,
        Value::Rect(std::sync::Arc::new(Rect {
            x: l.x,
            y: l.y,
            width: 0.0,
            height: 0.0,
        })),
    );
}

fn press_point(engine: &Engine, area: ObjectId, p: ObjectId, f: &Finger) {
    let id = match f.who {
        Who::Mouse => 0,
        Who::Finger(PointerId(n)) => i32::try_from(n).unwrap_or(i32::MAX),
    };
    set(engine, p, TouchPoint::POINT_ID.index, Value::Int(id));
    place(engine, area, p, f.pos);
    // Qt 6.11: a point whose finger came down in an event the area did
    // not take has no start.
    let start = if f.began {
        local_point(engine, area, f.pos)
    } else {
        kurbo::Point::ZERO
    };
    let prev = local_point(engine, area, f.prev);
    for (prop, v) in [
        (TouchPoint::START_X.index, start.x),
        (TouchPoint::START_Y.index, start.y),
        (TouchPoint::PREVIOUS_X.index, prev.x),
        (TouchPoint::PREVIOUS_Y.index, prev.y),
        (TouchPoint::PRESSURE.index, 1.0),
    ] {
        real(engine, p, prop, v);
    }
    set(
        engine,
        p,
        TouchPoint::VELOCITY.index,
        Value::Vector2d(Vector2d::default()),
    );
    set(
        engine,
        p,
        TouchPoint::ELLIPSE_DIAMETERS.index,
        Value::Size(std::sync::Arc::new(Size::default())),
    );
    set(engine, p, TouchPoint::PRESSED.index, Value::Bool(true));
}

/// The point went from where it was to `to`, `dt` seconds after it last
/// moved. `prev` is where the finger was before its last move, which
/// may have come in an event the area did not take.
fn move_point(
    engine: &Engine,
    area: ObjectId,
    p: ObjectId,
    prev: kurbo::Point,
    to: kurbo::Point,
    dt: f64,
) {
    let read = |prop| num(engine, p, prop);
    let was = kurbo::Point::new(
        read(TouchPoint::SCENE_X.index),
        read(TouchPoint::SCENE_Y.index),
    );
    let before = local_point(engine, area, prev);
    real(engine, p, TouchPoint::PREVIOUS_X.index, before.x);
    real(engine, p, TouchPoint::PREVIOUS_Y.index, before.y);
    if dt > 0.0 {
        let v = (to - was) / dt;
        set(
            engine,
            p,
            TouchPoint::VELOCITY.index,
            Value::Vector2d(Vector2d {
                x: v.x as f32,
                y: v.y as f32,
            }),
        );
    }
    place(engine, area, p, to);
}

/// Whether the finger is further than the drag threshold from where it
/// came down, along either axis.
fn past_threshold(f: &Finger, threshold: f64) -> bool {
    (f.pos.x - f.down.x).abs() > threshold || (f.pos.y - f.down.y).abs() > threshold
}

/// Lets go of a finger without telling the area: what Qt 6.11 does when
/// the area is disabled or hidden while the finger is down. Its point is
/// left pressed.
pub(crate) fn forget(engine: &Engine, area: ObjectId, who: Who) {
    let Some(st) = state(engine, area) else {
        return;
    };
    let mut fingers = st.fingers.borrow_mut();
    let Some(at) = fingers.iter().position(|f| f.who == who) else {
        return;
    };
    let f = fingers.remove(at);
    if f.point.is_some_and(|p| st.made.borrow().contains(&p)) {
        st.spare.borrow_mut().extend(f.point);
    }
    if fingers.is_empty() {
        st.grabbed.set(false);
    }
}

/// Tells `area` what its pointers did in one event.
pub(crate) fn deliver(engine: &Engine, area: ObjectId, changes: &[Change]) {
    use crate::touch::TouchPhase;
    let Some(st) = state(engine, area) else {
        return;
    };
    {
        let mut fingers = st.fingers.borrow_mut();
        for f in fingers.iter_mut() {
            f.moved = false;
            f.ended = None;
            f.began = false;
        }
        for c in changes {
            match fingers.iter_mut().find(|f| f.who == c.who) {
                Some(f) if c.phase != TouchPhase::Begin => {
                    f.moved = f.pos != c.pos || c.phase != TouchPhase::Move;
                    if f.pos != c.pos {
                        f.prev = f.pos;
                    }
                    f.pos = c.pos;
                    f.seen = c.time;
                    if matches!(c.phase, TouchPhase::End | TouchPhase::Cancel) {
                        f.ended = Some(c.phase);
                    }
                }
                Some(_) => {}
                None if c.phase == TouchPhase::Begin => fingers.push(Finger {
                    who: c.who,
                    pos: c.pos,
                    prev: c.pos,
                    down: c.pos,
                    began: true,
                    point: None,
                    moved: false,
                    ended: None,
                    time: c.time,
                    seen: c.time,
                }),
                None => {}
            }
        }
    }
    let count = st.fingers.borrow().len();
    let least = num(
        engine,
        area,
        MultiPointTouchArea::MINIMUM_TOUCH_POINTS.index,
    );
    let most = num(
        engine,
        area,
        MultiPointTouchArea::MAXIMUM_TOUCH_POINTS.index,
    );
    let taken = (count as f64) >= least && (count as f64) <= most;

    let (mut released, mut canceled, mut updated, mut pressed) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    if taken {
        let n = st.fingers.borrow().len();
        for i in 0..n {
            let (point, ended, moved, pos, prev, time, now, who) = {
                let f = &st.fingers.borrow()[i];
                (
                    f.point, f.ended, f.moved, f.pos, f.prev, f.time, f.seen, f.who,
                )
            };
            match (point, ended) {
                (Some(p), Some(phase)) => {
                    let dt = now.saturating_since(time).as_secs_f64();
                    move_point(engine, area, p, prev, pos, dt);
                    set(engine, p, TouchPoint::PRESSED.index, Value::Bool(false));
                    // Qt 6.11: the mouse's point keeps its pressure once
                    // the button is up; a finger's goes to 0.
                    if who != Who::Mouse {
                        real(engine, p, TouchPoint::PRESSURE.index, 0.0);
                    }
                    if phase == TouchPhase::Cancel {
                        canceled.push(p);
                    } else {
                        released.push(p);
                    }
                }
                (None, Some(_)) => {}
                (None, None) => {
                    if let Some(p) = free_point(engine, area, &st) {
                        let mut fingers = st.fingers.borrow_mut();
                        let f = &mut fingers[i];
                        f.point = Some(p);
                        f.time = now;
                        press_point(engine, area, p, f);
                        pressed.push(p);
                    }
                }
                (Some(p), None) => {
                    // A finger can have moved in an event the area did not
                    // take: the point is where it is before it is told.
                    let told = kurbo::Point::new(
                        num(engine, p, TouchPoint::SCENE_X.index),
                        num(engine, p, TouchPoint::SCENE_Y.index),
                    );
                    if moved || told != pos {
                        let dt = now.saturating_since(time).as_secs_f64();
                        move_point(engine, area, p, prev, pos, dt);
                        st.fingers.borrow_mut()[i].time = now;
                    }
                    updated.push(p);
                }
            }
        }
    }
    // Qt 6.11: a finger lifted in an event the area did not take leaves
    // its point pressed; the point is free for the next finger all the
    // same.
    let left: Vec<ObjectId> = {
        let mut fingers = st.fingers.borrow_mut();
        let mut freed = Vec::new();
        fingers.retain(|f| {
            if f.ended.is_some() {
                freed.extend(f.point);
                false
            } else {
                true
            }
        });
        freed
    };
    let made = st.made.borrow().clone();
    st.spare
        .borrow_mut()
        .extend(left.into_iter().filter(|p| made.contains(p)));
    if !taken {
        if st.fingers.borrow().is_empty() {
            st.grabbed.set(false);
        }
        return;
    }
    let down: Vec<ObjectId> = st.fingers.borrow().iter().filter_map(|f| f.point).collect();
    let threshold = crate::system_prefs::start_drag_distance(engine);
    let moving = st
        .fingers
        .borrow()
        .iter()
        .any(|f| f.point.is_some_and(|p| updated.contains(&p)) && past_threshold(f, threshold));
    if !st.grabbed.get() && moving {
        let grabbed = Rc::new(Cell::new(false));
        let gesture = GestureEvent {
            grabbed: grabbed.clone(),
            drag_threshold: threshold,
            ..GestureEvent::default()
        };
        if let Ok(g) = engine.create_native_incomplete(gesture, Some(area)) {
            for &p in &down {
                let _ = engine.list_push(g, GestureEvent::TOUCH_POINTS.index, p);
            }
            emit(engine, area, "gestureStarted", &[Value::Object(g)]);
            engine.destroy_now(g);
        }
        st.grabbed.set(grabbed.get());
    }
    for (name, points) in [
        ("released", &released),
        ("canceled", &canceled),
        ("updated", &updated),
        ("pressed", &pressed),
    ] {
        if !points.is_empty() {
            emit(engine, area, name, &[list(points)]);
        }
    }
    if !(released.is_empty() && canceled.is_empty() && updated.is_empty() && pressed.is_empty()) {
        emit(engine, area, "touchUpdated", &[list(&down)]);
    }
    if st.fingers.borrow().is_empty() {
        st.grabbed.set(false);
    }
}

fn mouse(area: ObjectId, engine: &Engine, phase: crate::touch::TouchPhase, ev: &PointerEvent) {
    deliver(
        engine,
        area,
        &[Change {
            who: Who::Mouse,
            phase,
            pos: ev.pos,
            time: ev.time,
        }],
    );
}

fn mouse_press(engine: &Engine, area: ObjectId, ev: &PointerEvent, _: kurbo::Point) -> bool {
    if ev.button != Some(MouseButtons::LEFT) {
        return false;
    }
    mouse(area, engine, crate::touch::TouchPhase::Begin, ev);
    true
}

fn mouse_moved(engine: &Engine, area: ObjectId, ev: &PointerEvent, _: kurbo::Point) {
    mouse(area, engine, crate::touch::TouchPhase::Move, ev);
}

fn mouse_release(engine: &Engine, area: ObjectId, ev: &PointerEvent, _: kurbo::Point) {
    mouse(area, engine, crate::touch::TouchPhase::End, ev);
}

fn mouse_cancel(engine: &Engine, area: ObjectId) {
    let Some(st) = state(engine, area) else {
        return;
    };
    let at = st
        .fingers
        .borrow()
        .iter()
        .find(|f| f.who == Who::Mouse)
        .map(|f| (f.pos, f.time));
    if let Some((pos, time)) = at {
        deliver(
            engine,
            area,
            &[Change {
                who: Who::Mouse,
                phase: crate::touch::TouchPhase::Cancel,
                pos,
                time,
            }],
        );
    }
}

fn takes_mouse(engine: &Engine, area: ObjectId) -> bool {
    flag(engine, area, MultiPointTouchArea::MOUSE_ENABLED.index)
        && flag(engine, area, Item::ENABLED.index)
}

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [
        MultiPointTouchArea, TouchPoint, GestureEvent,
    ]));
    let meta = MetaRef::Static(MultiPointTouchArea::static_meta());
    crate::pointer_item::register_pointer_item(
        engine,
        meta.clone(),
        crate::pointer_item::PointerItem {
            press: mouse_press,
            moved: mouse_moved,
            release: mouse_release,
            cancel: mouse_cancel,
            hover: |_, _, _| {},
        },
    );
    crate::pointer_item::register_pointer_condition(engine, meta, takes_mouse);
}
