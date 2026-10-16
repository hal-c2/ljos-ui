//! `Flickable`: a view onto a surface larger than itself.
//!
//! The children live on the content item, which the Flickable moves the
//! other way of `contentX`/`contentY`. What is outside the view is culled
//! by the render tree's composer (ARCHITECTURE §8.2): a clipping
//! Flickable cuts its content to its own rectangle, and one that does not
//! clip still loses what is outside the window. Culled items are not hit
//! either, since hit testing reads the composed frame.
//!
//! The pointer drags the content ([`drag`]) and the wheel moves it
//! ([`wheel`]). What the content does when nothing holds it is asked of a
//! [`ScrollPhysics`] ([`physics`]) as curves, which [`fling`] follows frame
//! by frame: by default it keeps going, slows down, gives way past its
//! bounds and comes back, as on a desktop Qt.

mod a11y;
pub mod drag;
pub(crate) mod fling;
pub mod physics;
pub mod wheel;

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, LogLevel, MetaRef, ObjectId, Point, PropIndex, QmlList, QmlObject,
    QmlType, Timer, Value, Version, WriteKind, qml_impl,
};

use crate::enums::EnumTable;
use crate::item::{Item, is_a, is_item, is_unset};
use crate::tree;

pub use a11y::{scroll_by_action, scroll_extent};
pub use drag::{keep_pointer_grab, keeps_pointer_grab};
pub use physics::{
    AxisState, FlickSettings, Kinetic, Motion, ScrollPhysics, Segment, VelocityTracker,
    WHEEL_NOTCH_PIXELS, scroll_physics, set_scroll_physics,
};
pub use wheel::{WheelItem, register_wheel_item};

/// The log category of what a Flickable could not do.
pub const CATEGORY: &str = "qt.quick.flickable";

/// `Flickable.flickableDirection`.
pub mod flickable_direction {
    pub const AUTO: i32 = 0;
    pub const HORIZONTAL: i32 = 1;
    pub const VERTICAL: i32 = 2;
    pub const HORIZONTAL_AND_VERTICAL: i32 = 3;
    pub const AUTO_IF_NEEDED: i32 = 12;
}

/// `Flickable.boundsBehavior`, flags: whether a drag, and whether a
/// fling, may take the content past its bounds.
pub mod bounds_behavior {
    pub const STOP_AT_BOUNDS: i32 = 0;
    pub const DRAG_OVER_BOUNDS: i32 = 1;
    pub const OVERSHOOT_BOUNDS: i32 = 2;
    pub const DRAG_AND_OVERSHOOT_BOUNDS: i32 = 3;
}

/// `Flickable.boundsMovement`.
pub mod bounds_movement {
    pub const STOP_AT_BOUNDS: i32 = 0;
    pub const FOLLOW_BOUNDS_BEHAVIOR: i32 = 1;
}

fn keys(list: &[(&'static str, i32)]) -> Vec<(&'static str, i64)> {
    list.iter().map(|(k, v)| (*k, i64::from(*v))).collect()
}

fn flickable_enums() -> EnumTable {
    use bounds_behavior as b;
    use bounds_movement as m;
    use flickable_direction as d;
    vec![
        (
            "FlickableDirection",
            keys(&[
                ("AutoFlickDirection", d::AUTO),
                ("HorizontalFlick", d::HORIZONTAL),
                ("VerticalFlick", d::VERTICAL),
                ("HorizontalAndVerticalFlick", d::HORIZONTAL_AND_VERTICAL),
                ("AutoFlickIfNeeded", d::AUTO_IF_NEEDED),
            ]),
        ),
        (
            "BoundsBehavior",
            keys(&[
                ("StopAtBounds", b::STOP_AT_BOUNDS),
                ("DragOverBounds", b::DRAG_OVER_BOUNDS),
                ("OvershootBounds", b::OVERSHOOT_BOUNDS),
                ("DragAndOvershootBounds", b::DRAG_AND_OVERSHOOT_BOUNDS),
            ]),
        ),
        (
            // `StopAtBounds` is in both enums with the same value, so the
            // one above answers for it.
            "BoundsMovement",
            keys(&[("FollowBoundsBehavior", m::FOLLOW_BOUNDS_BEHAVIOR)]),
        ),
    ]
}

/// `Flickable` (QtQuick).
///
/// What the content does when it is let go is the engine's
/// [`ScrollPhysics`]; with [`ScrollPhysics::Immediate`] it stops when the
/// finger or the wheel does and never leaves its bounds.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Flickable",
    extends = Item,
    default = "flickableData",
    enums = flickable_enums
)]
pub struct Flickable {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub content_x: f64,
    #[qml(notify)]
    pub content_y: f64,
    /// Negative: not given, and the content is as wide as the Flickable.
    #[qml(notify)]
    pub content_width: f64,
    #[qml(notify)]
    pub content_height: f64,
    /// The item the children are on. Made when the first child arrives or
    /// at completion, whichever is first.
    #[qml(notify, readonly)]
    pub content_item: Option<ObjectId>,
    /// What was declared inside the Flickable. An `Item` goes onto the
    /// content item, anything else is a resource of the Flickable.
    #[qml(notify, list, append = flickable_append)]
    pub flickable_data: QmlList,
    #[qml(notify, list, append = flickable_append)]
    pub flickable_children: QmlList,
    #[qml(notify, readonly)]
    pub origin_x: f64,
    #[qml(notify, readonly)]
    pub origin_y: f64,
    #[qml(notify)]
    pub top_margin: f64,
    #[qml(notify)]
    pub bottom_margin: f64,
    #[qml(notify)]
    pub left_margin: f64,
    #[qml(notify)]
    pub right_margin: f64,
    #[qml(notify)]
    pub interactive: bool,
    /// One of [`flickable_direction`].
    #[qml(notify, enumeration = "FlickableDirection")]
    pub flickable_direction: i32,
    /// One of [`bounds_behavior`].
    #[qml(notify, flags = "BoundsBehavior")]
    pub bounds_behavior: i32,
    /// One of [`bounds_movement`].
    #[qml(notify, enumeration = "BoundsMovement")]
    pub bounds_movement: i32,
    #[qml(notify, readonly)]
    pub at_x_beginning: bool,
    #[qml(notify, readonly)]
    pub at_x_end: bool,
    #[qml(notify, readonly)]
    pub at_y_beginning: bool,
    #[qml(notify, readonly)]
    pub at_y_end: bool,
    #[qml(notify, readonly)]
    pub moving: bool,
    #[qml(notify, readonly)]
    pub moving_horizontally: bool,
    #[qml(notify, readonly)]
    pub moving_vertically: bool,
    #[qml(notify, readonly)]
    pub dragging: bool,
    #[qml(notify, readonly)]
    pub dragging_horizontally: bool,
    #[qml(notify, readonly)]
    pub dragging_vertically: bool,
    #[qml(notify, readonly)]
    pub flicking: bool,
    #[qml(notify, readonly)]
    pub flicking_horizontally: bool,
    #[qml(notify, readonly)]
    pub flicking_vertically: bool,
    #[qml(notify, readonly)]
    pub horizontal_velocity: f64,
    #[qml(notify, readonly)]
    pub vertical_velocity: f64,
    #[qml(notify, readonly)]
    pub horizontal_overshoot: f64,
    #[qml(notify, readonly)]
    pub vertical_overshoot: f64,
    /// Kept; a press is delivered at once.
    #[qml(notify)]
    pub press_delay: i32,
    #[qml(notify)]
    pub flick_deceleration: f64,
    #[qml(notify)]
    pub maximum_flick_velocity: f64,
    #[qml(notify)]
    pub pixel_aligned: bool,
    #[qml(notify)]
    pub synchronous_drag: bool,
    /// The transition that brings the content back inside its bounds,
    /// instead of the physics' own rebound.
    #[qml(notify)]
    pub rebound: Option<ObjectId>,
    #[qml(group)]
    pub visible_area: FlickableVisibleArea,
    #[qml(skip)]
    pub(crate) state: Rc<FlickState>,
}

impl Default for Flickable {
    fn default() -> Self {
        Self {
            item: Item::default(),
            content_x: 0.0,
            content_y: 0.0,
            content_width: -1.0,
            content_height: -1.0,
            content_item: None,
            flickable_data: QmlList::default(),
            flickable_children: QmlList::default(),
            origin_x: 0.0,
            origin_y: 0.0,
            top_margin: 0.0,
            bottom_margin: 0.0,
            left_margin: 0.0,
            right_margin: 0.0,
            interactive: true,
            flickable_direction: flickable_direction::AUTO,
            bounds_behavior: bounds_behavior::DRAG_AND_OVERSHOOT_BOUNDS,
            bounds_movement: bounds_movement::FOLLOW_BOUNDS_BEHAVIOR,
            at_x_beginning: true,
            at_x_end: true,
            at_y_beginning: true,
            at_y_end: true,
            moving: false,
            moving_horizontally: false,
            moving_vertically: false,
            dragging: false,
            dragging_horizontally: false,
            dragging_vertically: false,
            flicking: false,
            flicking_horizontally: false,
            flicking_vertically: false,
            horizontal_velocity: 0.0,
            vertical_velocity: 0.0,
            horizontal_overshoot: 0.0,
            vertical_overshoot: 0.0,
            press_delay: 0,
            flick_deceleration: 1500.0,
            maximum_flick_velocity: 2500.0,
            pixel_aligned: false,
            synchronous_drag: false,
            rebound: None,
            visible_area: FlickableVisibleArea::default(),
            state: Rc::default(),
        }
    }
}

/// The `visibleArea` group of [`Flickable`]: which share of the content is
/// seen and where it starts, each between 0 and 1. What a scroll bar
/// shows.
#[derive(Debug, QmlObject)]
#[qml(name = "FlickableVisibleArea", uncreatable)]
pub struct FlickableVisibleArea {
    #[qml(notify, readonly)]
    pub x_position: f64,
    #[qml(notify, readonly)]
    pub y_position: f64,
    #[qml(notify, readonly)]
    pub width_ratio: f64,
    #[qml(notify, readonly)]
    pub height_ratio: f64,
}

impl Default for FlickableVisibleArea {
    fn default() -> Self {
        Self {
            x_position: 0.0,
            y_position: 0.0,
            width_ratio: 1.0,
            height_ratio: 1.0,
        }
    }
}

/// What a Flickable remembers between events. Not properties: nothing
/// binds to it.
#[derive(Debug)]
pub(crate) struct FlickState {
    content_item: Cell<Option<ObjectId>>,
    /// Set while `resizeContent` writes, so that the new size does not
    /// bring a position inside that the method is about to give.
    resizing: Cell<bool>,
    /// Which directions the wheel, and which the pointer, are moving.
    wheel_moving: Cell<[bool; 2]>,
    drag_moving: Cell<[bool; 2]>,
    /// The timer that says the wheel has stopped.
    wheel_timer: Cell<Option<ObjectId>>,
    /// False when whoever wraps the Flickable keeps the wheel from it.
    wheel_enabled: Cell<bool>,
    /// A view whose rows run the other way in a direction (a list from
    /// the bottom up) says so here: its content then ends at the origin
    /// side and what is short of the view sits at the far end. Never set
    /// for a plain Flickable.
    reversed: Cell<[bool; 2]>,
    pub(crate) drag: drag::DragState,
    pub(crate) fling: fling::FlingState,
}

impl Default for FlickState {
    fn default() -> Self {
        Self {
            content_item: Cell::new(None),
            resizing: Cell::new(false),
            wheel_moving: Cell::new([false; 2]),
            drag_moving: Cell::new([false; 2]),
            wheel_timer: Cell::new(None),
            wheel_enabled: Cell::new(true),
            reversed: Cell::new([false; 2]),
            drag: drag::DragState::default(),
            fling: fling::FlingState::default(),
        }
    }
}

/// The two directions, as indexes into the per-direction state.
pub(crate) const X: usize = 0;
pub(crate) const Y: usize = 1;

/// The properties of one direction.
pub(crate) struct Dir {
    pub position: PropIndex,
    pub content: PropIndex,
    /// Where the content starts: 0 unless a view moved it.
    pub origin: PropIndex,
    pub view: PropIndex,
    pub lead: PropIndex,
    pub trail: PropIndex,
    pub at_beginning: PropIndex,
    pub at_end: PropIndex,
    pub moving: PropIndex,
    pub dragging: PropIndex,
    pub forced_by: i32,
}

pub(crate) fn dir(axis: usize) -> Dir {
    if axis == X {
        Dir {
            position: Flickable::CONTENT_X.index,
            content: Flickable::CONTENT_WIDTH.index,
            origin: Flickable::ORIGIN_X.index,
            view: Item::WIDTH.index,
            lead: Flickable::LEFT_MARGIN.index,
            trail: Flickable::RIGHT_MARGIN.index,
            at_beginning: Flickable::AT_X_BEGINNING.index,
            at_end: Flickable::AT_X_END.index,
            moving: Flickable::MOVING_HORIZONTALLY.index,
            dragging: Flickable::DRAGGING_HORIZONTALLY.index,
            forced_by: flickable_direction::HORIZONTAL,
        }
    } else {
        Dir {
            position: Flickable::CONTENT_Y.index,
            content: Flickable::CONTENT_HEIGHT.index,
            origin: Flickable::ORIGIN_Y.index,
            view: Item::HEIGHT.index,
            lead: Flickable::TOP_MARGIN.index,
            trail: Flickable::BOTTOM_MARGIN.index,
            at_beginning: Flickable::AT_Y_BEGINNING.index,
            at_end: Flickable::AT_Y_END.index,
            moving: Flickable::MOVING_VERTICALLY.index,
            dragging: Flickable::DRAGGING_VERTICALLY.index,
            forced_by: flickable_direction::VERTICAL,
        }
    }
}

pub(crate) fn num(e: &Engine, obj: ObjectId, p: PropIndex) -> f64 {
    e.read(obj, p).to_number()
}

pub(crate) fn flag(e: &Engine, obj: ObjectId, p: PropIndex) -> bool {
    e.read(obj, p).to_boolean()
}

pub(crate) fn warn(e: &Engine, what: &str, r: Result<(), EngineError>) {
    if let Err(err) = r {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("Flickable: {what}: {err}"),
        );
    }
}

/// Whether `obj` is a `Flickable` or derives from it.
pub fn is_flickable(engine: &Engine, obj: ObjectId) -> bool {
    is_a::<Flickable>(engine, obj)
}

pub(crate) fn state_of(engine: &Engine, obj: ObjectId) -> Option<Rc<FlickState>> {
    engine.with(obj, |f: &Flickable| f.state.clone()).ok()
}

/// A native write that is not an assignment: the value is the same as
/// before or the property takes it, and a binding the author put there
/// gives its value again when it next runs.
pub(crate) fn put(e: &Engine, obj: ObjectId, p: PropIndex, v: Value) {
    if e.untrack(|| e.read(obj, p)).change_equals(&v) {
        return;
    }
    warn(e, "write", e.write_internal(obj, p, v, WriteKind::Layout));
}

pub(crate) fn put_flag(e: &Engine, obj: ObjectId, p: PropIndex, v: bool) {
    put(e, obj, p, Value::Bool(v));
}

/// How long the content is in one direction: the content size, or the
/// Flickable's own when none was given. Reads are tracked.
pub(crate) fn extent(e: &Engine, f: ObjectId, d: &Dir) -> f64 {
    let content = num(e, f, d.content);
    if content < 0.0 {
        num(e, f, d.view)
    } else {
        content
    }
}

/// Where the content is in one direction and how far it can go. Reads are
/// tracked when a binding or an effect asks.
pub fn axis_state(engine: &Engine, flickable: ObjectId, horizontal: bool) -> AxisState {
    bounds(engine, flickable, if horizontal { X } else { Y })
}

pub(crate) fn bounds(e: &Engine, f: ObjectId, axis: usize) -> AxisState {
    let d = dir(axis);
    // The origin is 0 for a plain Flickable; a view moves it when rows
    // come in before the first one it laid out.
    let origin = num(e, f, d.origin);
    let (lead, trail) = (num(e, f, d.lead), num(e, f, d.trail));
    let far = origin + extent(e, f, &d) - num(e, f, d.view) + trail;
    let reversed = e
        .with(f, |fl: &Flickable| fl.state.reversed.get()[axis])
        .unwrap_or(false);
    let (min, max) = if reversed {
        (far.min(origin - lead), far)
    } else {
        (origin - lead, far.max(origin - lead))
    };
    AxisState {
        position: num(e, f, d.position),
        min,
        max,
        velocity: 0.0,
    }
}

/// Whether the pointer and the wheel can move the content in a direction.
pub(crate) fn can_flick(e: &Engine, f: ObjectId, axis: usize) -> bool {
    use flickable_direction as fd;
    let d = dir(axis);
    let content = num(e, f, d.content);
    let view = num(e, f, d.view);
    match num(e, f, Flickable::FLICKABLE_DIRECTION.index) as i32 {
        fd::AUTO => content >= 0.0 && content != view,
        fd::AUTO_IF_NEEDED => content > view,
        fd::HORIZONTAL_AND_VERTICAL => true,
        other => other == d.forced_by,
    }
}

/// Puts the content at `to` in one direction, as scrolling does: no
/// assignment, so a binding on the position stays.
pub(crate) fn scroll_to(e: &Engine, f: ObjectId, axis: usize, to: f64) {
    put(e, f, dir(axis).position, Value::Real(to));
}

/// Moves the content of `flickable` to `position` in one direction, as a
/// scroll bar does. Nothing is brought inside the bounds: the caller
/// knows them ([`axis_state`]).
pub fn scroll(engine: &Engine, flickable: ObjectId, horizontal: bool, position: f64) {
    if is_flickable(engine, flickable) {
        scroll_to(engine, flickable, if horizontal { X } else { Y }, position);
    }
}

/// The item the children of `flickable` are on, made now if it was not
/// there yet. What is given the Flickable as a parent afterwards (a
/// scroll bar) is then above the content, as it has to be.
pub fn content_item(engine: &Engine, flickable: ObjectId) -> Option<ObjectId> {
    if !is_flickable(engine, flickable) {
        return None;
    }
    ensure_content_item(engine, flickable).ok()
}

/// Whether the wheel moves the content of `flickable`. A view that wraps
/// a Flickable says so here when its own `wheelEnabled` changes.
pub fn set_wheel_enabled(engine: &Engine, flickable: ObjectId, on: bool) {
    if let Some(st) = state_of(engine, flickable) {
        st.wheel_enabled.set(on);
    }
}

/// Rounds where the content is in one direction to a whole pixel if the
/// Flickable asks for that. A half goes away from zero, as on Qt.
fn align(e: &Engine, f: ObjectId, axis: usize) {
    e.untrack(|| {
        if !flag(e, f, Flickable::PIXEL_ALIGNED.index) {
            return;
        }
        let at = num(e, f, dir(axis).position);
        if at.is_finite() && at != at.round() {
            scroll_to(e, f, axis, at.round());
        }
    });
}

/// Says that the content of `f` runs the other way in one direction
/// (see [`FlickState`]). For the views.
pub(crate) fn set_reversed(e: &Engine, f: ObjectId, axis: usize, reversed: bool) {
    if let Some(st) = state_of(e, f) {
        let mut now = st.reversed.get();
        now[axis] = reversed;
        st.reversed.set(now);
    }
}

/// Brings the content inside its bounds in one direction. Content that
/// moves by itself or follows the pointer is left alone: what moves it
/// takes the new bounds into account.
pub(crate) fn settle(e: &Engine, f: ObjectId, axis: usize) {
    if let Some(st) = state_of(e, f)
        && (st.fling.axes()[axis] || drag::follows(&st, axis))
    {
        return;
    }
    let state = e.untrack(|| bounds(e, f, axis));
    if let Motion::Jump(to) = scroll_physics(e).settle(&state) {
        scroll_to(e, f, axis, to);
    }
}

/// Brings `moving` and the per-direction flags in step with what the
/// wheel, the pointer and a fling are doing, and says when movement
/// starts and ends.
pub(crate) fn sync_moving(e: &Engine, f: ObjectId, st: &FlickState) {
    let (wheel, drag) = (st.wheel_moving.get(), st.drag_moving.get());
    let fling = st.fling.moving();
    let was = e.untrack(|| flag(e, f, Flickable::MOVING.index));
    let now = [X, Y].map(|a| wheel[a] || drag[a] || fling[a]);
    let any = now[X] || now[Y];
    e.batch(|| {
        for axis in [X, Y] {
            put_flag(e, f, dir(axis).moving, now[axis]);
        }
        put_flag(e, f, Flickable::MOVING.index, any);
    });
    if any != was {
        let signal = if any {
            "movementStarted"
        } else {
            "movementEnded"
        };
        warn(e, signal, e.emit_by_name(f, signal, &[]));
    }
}

pub(crate) fn set_drag_moving(e: &Engine, f: ObjectId, st: &FlickState, moving: [bool; 2]) {
    st.drag_moving.set(moving);
    sync_moving(e, f, st);
}

/// The item the children of `f` are on, made if it is not there yet.
fn ensure_content_item(e: &Engine, f: ObjectId) -> Result<ObjectId, EngineError> {
    let st = e.with(f, |fl: &Flickable| fl.state.clone())?;
    if let Some(ci) = st.content_item.get().filter(|ci| e.is_alive(*ci)) {
        return Ok(ci);
    }
    let ci = e.create_native_incomplete(Item::default(), Some(f))?;
    e.complete(ci)?;
    st.content_item.set(Some(ci));
    tree::set_parent_item(e, ci, Some(f))?;
    // The content item is the Flickable's to place and to size, so these
    // are values it computes, not assignments.
    e.bind_computed(ci, Item::X.index, move |e| {
        Value::Real(-num(e, f, Flickable::CONTENT_X.index))
    })?;
    e.bind_computed(ci, Item::Y.index, move |e| {
        Value::Real(-num(e, f, Flickable::CONTENT_Y.index))
    })?;
    e.bind_computed(ci, Item::WIDTH.index, move |e| {
        Value::Real(extent(e, f, &dir(X)))
    })?;
    e.bind_computed(ci, Item::HEIGHT.index, move |e| {
        Value::Real(extent(e, f, &dir(Y)))
    })?;
    e.set_internal(f, Flickable::CONTENT_ITEM, Some(ci))?;
    Ok(ci)
}

/// The append hook of `flickableData` and `flickableChildren`.
fn flickable_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    let (e, f) = (cx.engine, cx.this);
    if is_item(e, child) {
        let ci = ensure_content_item(e, f)?;
        tree::set_parent_item(e, child, Some(ci))?;
        e.list_push(f, Flickable::FLICKABLE_CHILDREN.index, child)?;
    } else {
        // A handler or a timer declared in the Flickable is the
        // Flickable's own, where the scene looks for handlers.
        tree::add_data(e, f, child)?;
    }
    e.list_push(f, Flickable::FLICKABLE_DATA.index, child)
}

/// Gives the four `at…` flags their first values without a change
/// signal. In Qt a Flickable that is declared away from an end says so
/// from the start and `onAtYEndChanged` does not run for it (checked on
/// qml6); a handler is there before completion in Ljós, so a plain write
/// would run it.
fn start_at_flags(e: &Engine, f: ObjectId) {
    use ljos_reactive::NodeKind;
    let g = e.graph();
    for axis in [X, Y] {
        let d = dir(axis);
        let s = e.untrack(|| bounds(e, f, axis));
        for (p, v, begin) in [
            (d.at_beginning, s.position <= s.min, true),
            (d.at_end, s.position >= s.max, false),
        ] {
            if e.untrack(|| flag(e, f, p)) == v {
                continue;
            }
            // A throwaway watch is how the property's node is found.
            let node = e.watch(f, p, |_, _| {}).ok().and_then(|w| {
                let node = g.sources(w).first().copied();
                let _ = g.dispose(w);
                node
            });
            // A binding that read the flag already would keep the old
            // value: then the flag changes the usual way.
            let quiet = node.is_some_and(|n| {
                g.observers(n)
                    .iter()
                    .all(|o| g.kind(*o) == Some(NodeKind::Effect))
                    && g.replace_quietly(n, Value::Bool(v)).unwrap_or(false)
            });
            if !quiet {
                put_flag(e, f, p, v);
                continue;
            }
            let _ = e.with_mut(f, |fl: &mut Flickable| match (axis == X, begin) {
                (true, true) => fl.at_x_beginning = v,
                (true, false) => fl.at_x_end = v,
                (false, true) => fl.at_y_beginning = v,
                (false, false) => fl.at_y_end = v,
            });
        }
    }
}

fn visible_area_props() -> Option<[[PropIndex; 2]; 2]> {
    let meta = Flickable::static_meta();
    let p = |name| meta.property_index(name);
    Some([
        [p("visibleArea.xPosition")?, p("visibleArea.widthRatio")?],
        [p("visibleArea.yPosition")?, p("visibleArea.heightRatio")?],
    ])
}

/// The seen share of one direction and where it starts. The margins count
/// as content.
fn visible_share(e: &Engine, f: ObjectId, axis: usize) -> (f64, f64) {
    let d = dir(axis);
    let view = num(e, f, d.view);
    let lead = num(e, f, d.lead);
    let total = (extent(e, f, &d) + lead + num(e, f, d.trail)).max(view);
    if total <= 0.0 {
        return (0.0, 1.0);
    }
    let from = num(e, f, d.position) - num(e, f, d.origin);
    ((from + lead) / total, view / total)
}

#[qml_impl]
impl Flickable {
    #[qml(signal, name = "movementStarted")]
    fn movement_started();

    #[qml(signal, name = "movementEnded")]
    fn movement_ended();

    #[qml(signal, name = "flickStarted")]
    fn flick_started();

    #[qml(signal, name = "flickEnded")]
    fn flick_ended();

    #[qml(signal, name = "dragStarted")]
    fn drag_started();

    #[qml(signal, name = "dragEnded")]
    fn drag_ended();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        ensure_content_item(e, me)?;
        let st = e.with(me, |f: &Flickable| f.state.clone())?;
        for axis in [X, Y] {
            let d = dir(axis);
            // Content starts after the margin unless a position was
            // declared. A margin that changes later moves nothing.
            let lead = e.untrack(|| num(e, me, d.lead));
            if lead != 0.0 && is_unset(e, me, d.position) {
                scroll_to(e, me, axis, -lead);
            }
            // Read once so that a size that is bound has its first value
            // before the watches below are there: only a later change
            // brings the content inside, never what was declared.
            e.untrack(|| (num(e, me, d.content), num(e, me, d.view)));
            for p in [d.content, d.view] {
                let st = st.clone();
                e.watch(me, p, move |e, _| {
                    if !st.resizing.get() {
                        settle(e, me, axis);
                    }
                })?;
            }
            // `pixelAligned`: the content rests on whole pixels, whoever
            // put it where it is.
            e.watch(me, d.position, move |e, _| align(e, me, axis))?;
        }
        e.watch(me, Flickable::PIXEL_ALIGNED.index, move |e, _| {
            for axis in [X, Y] {
                align(e, me, axis);
            }
        })?;
        start_at_flags(e, me);
        e.effect_in(me, move |e| {
            for axis in [X, Y] {
                let d = dir(axis);
                let s = bounds(e, me, axis);
                put_flag(e, me, d.at_beginning, s.position <= s.min);
                put_flag(e, me, d.at_end, s.position >= s.max);
            }
        })?;
        if let Some(props) = visible_area_props() {
            for axis in [X, Y] {
                e.bind_initial(me, props[axis][0], move |e| {
                    Value::Real(visible_share(e, me, axis).0)
                })?;
                e.bind_initial(me, props[axis][1], move |e| {
                    Value::Real(visible_share(e, me, axis).1)
                })?;
            }
        }
        Ok(())
    }

    /// `returnToBounds()`: content that is outside its bounds comes back
    /// inside, with the rebound. Neither `flicking` nor `moving` shows it
    /// (measured on qml6).
    #[qml(invokable, name = "returnToBounds")]
    fn return_to_bounds(cx: &Cx) {
        fling::return_to_bounds(cx.engine, cx.this);
    }

    /// `resizeContent(width, height, center)`: gives the content a new
    /// size and keeps the point `center` of it where it is in the view.
    #[qml(invokable, name = "resizeContent")]
    fn resize_content(cx: &Cx, width: f64, height: f64, center: Point) {
        let (e, me) = (cx.engine, cx.this);
        let Some(st) = state_of(e, me) else { return };
        let old = e.untrack(|| {
            [X, Y].map(|axis| {
                let d = dir(axis);
                (extent(e, me, &d), num(e, me, d.position))
            })
        });
        st.resizing.set(true);
        let wrote = e.batch(|| -> Result<(), EngineError> {
            for (axis, size, at) in [(X, width, center.x), (Y, height, center.y)] {
                let d = dir(axis);
                e.write(me, d.content, Value::Real(size), WriteKind::Imperative)?;
                let (was, position) = old[axis];
                if was > 0.0 && size != was {
                    scroll_to(e, me, axis, position + at * size / was - at);
                }
            }
            Ok(())
        });
        st.resizing.set(false);
        warn(e, "resizeContent", wrote);
    }

    /// `flick(xVelocity, yVelocity)`: the content is flung as a finger
    /// would fling it at that velocity. A positive velocity moves the
    /// content towards its beginning, as a finger moving that way does.
    #[qml(invokable)]
    fn flick(cx: &Cx, x_velocity: f64, y_velocity: f64) {
        fling::flick(cx.engine, cx.this, [-x_velocity, -y_velocity]);
    }

    /// `cancelFlick()`: content that moves by itself stops where it is.
    #[qml(invokable, name = "cancelFlick")]
    fn cancel_flick(cx: &Cx) {
        fling::stop_all(cx.engine, cx.this);
    }
}

fn start_wheel_timer(e: &Engine, f: ObjectId, st: &FlickState, ms: i32) {
    let timer = match st.wheel_timer.get().filter(|t| e.is_alive(*t)) {
        Some(t) => t,
        None => {
            let made = e
                .create_native_incomplete(Timer::default(), Some(f))
                .and_then(|t| {
                    e.complete(t)?;
                    e.connect_by_name(t, "triggered", move |e, _| wheel_rested(e, f))?;
                    Ok(t)
                });
            match made {
                Ok(t) => {
                    st.wheel_timer.set(Some(t));
                    t
                }
                Err(err) => {
                    warn(e, "timer", Err(err));
                    return;
                }
            }
        }
    };
    let r = e
        .set(timer, Timer::INTERVAL, ms.max(1))
        .and_then(|()| e.invoke_by_name(timer, "restart", &[]).map(|_| ()));
    warn(e, "timer", r);
}

/// The wheel has stopped: nothing it moved is moving any more.
pub(crate) fn wheel_rested(e: &Engine, f: ObjectId) {
    let Some(st) = state_of(e, f) else { return };
    if let Some(t) = st.wheel_timer.get().filter(|t| e.is_alive(*t)) {
        let _ = e.set(t, Timer::RUNNING, false);
    }
    if st.wheel_moving.get() != [false; 2] {
        st.wheel_moving.set([false; 2]);
        sync_moving(e, f, &st);
    }
}

/// The wheel moved the content in these directions; it counts as moving
/// until the wheel says it stopped or has been quiet for `rest_ms`.
pub(crate) fn wheel_moved(e: &Engine, f: ObjectId, moved: [bool; 2], rest_ms: Option<i32>) {
    let Some(st) = state_of(e, f) else { return };
    let was = st.wheel_moving.get();
    st.wheel_moving
        .set([was[X] || moved[X], was[Y] || moved[Y]]);
    sync_moving(e, f, &st);
    match rest_ms {
        Some(ms) => start_wheel_timer(e, f, &st, ms),
        None => wheel_rested(e, f),
    }
}

pub(crate) fn wheel_enabled(e: &Engine, f: ObjectId) -> bool {
    state_of(e, f).is_some_and(|st| st.wheel_enabled.get())
}

/// Registers what makes a `Flickable` take the pointer and the wheel.
pub(crate) fn register(engine: &Engine) {
    let meta = MetaRef::Static(Flickable::static_meta());
    engine.register_type("QtQuick", Version::new(2, 15), meta.clone());
    crate::pointer_item::register_pointer_item(engine, meta.clone(), drag::hooks());
    wheel::register_wheel_item(engine, meta.clone(), wheel::hooks());
    crate::accessible::register_accessible_item(engine, meta, a11y::hooks());
}
