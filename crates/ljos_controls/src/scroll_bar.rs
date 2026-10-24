//! `ScrollBar` and `ScrollIndicator` (QtQuick.Templates): which part of
//! something larger is in view.
//!
//! Both hold a `position` and a `size`, each a share of the whole between
//! 0 and 1, and place their content item, the handle, along themselves to
//! show the two. A scroll bar also takes the pointer: its handle is
//! dragged and its track pressed. An indicator only shows.
//!
//! Attached to a Flickable (`ScrollBar.vertical: ScrollBar {}`), or to a
//! view whose content item is one, a bar follows the content and the
//! content follows the bar ([`follow`]).
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-scrollbar.html and
//! doc.qt.io/qt-6/qml-qtquick-controls-scrollindicator.html. The numbers
//! (where a press on the track puts the handle, what a step snaps to)
//! were learned by running `qml6` (Qt 6.11.2);
//! `features/controls/scroll-bar.feature` and `scroll-indicator.feature`
//! pin them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_a11y::{NodeData, Range, Role};
use ljos_engine::{
    Cx, Engine, EngineError, MetaRef, ObjectId, OverrideContent, OverrideId, PropIndex, QmlObject,
    QmlType, RestoreMode, Value, qml_impl,
};
use ljos_quick::{
    AccessibleAction, AccessibleItem, Flickable, Item, PointerEvent, PointerItem, flickable,
    is_item, is_mirrored, register_accessible_item, register_pointer_item, set_parent_item,
};
use ljos_render::kurbo;

use crate::control::{Control, hover, is_left, takes_input};
use crate::util::{check, flag, is_a, is_unset, num, object};

/// `Qt.Orientation`.
pub(crate) mod orientation {
    pub const HORIZONTAL: i32 = 1;
    pub const VERTICAL: i32 = 2;
}

/// `ScrollBar.policy`.
pub mod policy {
    pub const AS_NEEDED: i32 = 0;
    pub const ALWAYS_OFF: i32 = 1;
    pub const ALWAYS_ON: i32 = 2;
}

/// `ScrollBar.snapMode`.
pub mod snap_mode {
    pub const NO_SNAP: i32 = 0;
    pub const SNAP_ALWAYS: i32 = 1;
    pub const SNAP_ON_RELEASE: i32 = 2;
}

/// How far `increase()` and `decrease()` go when no `stepSize` was given.
const DEFAULT_STEP: f64 = 0.1;

fn scroll_bar_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    let keys = |list: &[(&'static str, i32)]| -> Vec<(&'static str, i64)> {
        list.iter().map(|(k, v)| (*k, i64::from(*v))).collect()
    };
    vec![
        (
            "Policy",
            keys(&[
                ("AsNeeded", policy::AS_NEEDED),
                ("AlwaysOff", policy::ALWAYS_OFF),
                ("AlwaysOn", policy::ALWAYS_ON),
            ]),
        ),
        (
            "SnapMode",
            keys(&[
                ("NoSnap", snap_mode::NO_SNAP),
                ("SnapAlways", snap_mode::SNAP_ALWAYS),
                ("SnapOnRelease", snap_mode::SNAP_ON_RELEASE),
            ]),
        ),
    ]
}

/// `ScrollBar`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "ScrollBar",
    extends = Control,
    enums = scroll_bar_enums,
    attached = ScrollBarAttached
)]
pub struct ScrollBar {
    #[qml(base)]
    pub control: Control,
    /// The share of the whole that is in view, between 0 and 1.
    #[qml(notify)]
    pub size: f64,
    /// Where the view starts, as a share of the whole. Kept as given: a
    /// position past the ends shows as a shorter handle.
    #[qml(notify)]
    pub position: f64,
    #[qml(notify)]
    pub step_size: f64,
    #[qml(notify)]
    pub active: bool,
    #[qml(notify)]
    pub pressed: bool,
    /// A `Qt.Orientation`.
    #[qml(notify, enumeration = "Qt.Orientation")]
    pub orientation: i32,
    /// One of [`snap_mode`].
    #[qml(notify, enumeration = "SnapMode")]
    pub snap_mode: i32,
    #[qml(notify)]
    pub interactive: bool,
    /// One of [`policy`]. The style shows and hides by it.
    #[qml(notify, enumeration = "Policy")]
    pub policy: i32,
    #[qml(notify, readonly)]
    pub horizontal: bool,
    #[qml(notify, readonly)]
    pub vertical: bool,
    #[qml(notify)]
    pub minimum_size: f64,
    #[qml(notify, readonly)]
    pub visual_size: f64,
    #[qml(notify, readonly)]
    pub visual_position: f64,
    #[qml(skip)]
    state: Rc<Held>,
}

impl Default for ScrollBar {
    fn default() -> Self {
        Self {
            control: Control::default(),
            size: 0.0,
            position: 0.0,
            step_size: 0.0,
            active: false,
            pressed: false,
            orientation: orientation::VERTICAL,
            snap_mode: snap_mode::NO_SNAP,
            interactive: true,
            policy: policy::AS_NEEDED,
            horizontal: false,
            vertical: true,
            minimum_size: 0.0,
            visual_size: 0.0,
            visual_position: 0.0,
            state: Rc::default(),
        }
    }
}

/// The press a scroll bar holds.
#[derive(Debug, Default)]
struct Held {
    held: Cell<bool>,
    /// Where on the handle the pointer has it, as a share of the track
    /// from the handle's start.
    offset: Cell<f64>,
}

/// What `ScrollBar.vertical` and `ScrollBar.horizontal` attach to a
/// Flickable or a view.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "ScrollBarAttached")]
pub struct ScrollBarAttached {
    #[qml(notify, holds = "ScrollBar")]
    pub horizontal: Option<ObjectId>,
    #[qml(notify, holds = "ScrollBar")]
    pub vertical: Option<ObjectId>,
}

/// `ScrollIndicator`: a scroll bar that only shows.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "ScrollIndicator",
    extends = Control,
    attached = ScrollIndicatorAttached
)]
pub struct ScrollIndicator {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub size: f64,
    #[qml(notify)]
    pub position: f64,
    #[qml(notify)]
    pub active: bool,
    /// A `Qt.Orientation`.
    #[qml(notify, enumeration = "Qt.Orientation")]
    pub orientation: i32,
    #[qml(notify, readonly)]
    pub horizontal: bool,
    #[qml(notify, readonly)]
    pub vertical: bool,
    #[qml(notify)]
    pub minimum_size: f64,
    #[qml(notify, readonly)]
    pub visual_size: f64,
    #[qml(notify, readonly)]
    pub visual_position: f64,
}

impl Default for ScrollIndicator {
    fn default() -> Self {
        Self {
            control: Control::default(),
            size: 0.0,
            position: 0.0,
            active: false,
            orientation: orientation::VERTICAL,
            horizontal: false,
            vertical: true,
            minimum_size: 0.0,
            visual_size: 0.0,
            visual_position: 0.0,
        }
    }
}

/// What `ScrollIndicator.vertical` and `ScrollIndicator.horizontal`
/// attach.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "ScrollIndicatorAttached")]
pub struct ScrollIndicatorAttached {
    #[qml(notify, holds = "ScrollIndicator")]
    pub horizontal: Option<ObjectId>,
    #[qml(notify, holds = "ScrollIndicator")]
    pub vertical: Option<ObjectId>,
}

/// The properties a bar and an indicator share, so that one piece of code
/// serves both.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bar {
    size: PropIndex,
    position: PropIndex,
    active: PropIndex,
    orientation: PropIndex,
    horizontal: PropIndex,
    vertical: PropIndex,
    minimum_size: PropIndex,
    visual_size: PropIndex,
    visual_position: PropIndex,
    /// Whether the pointer moves it, so that its position is something
    /// the content has to follow.
    moves_content: bool,
}

const SCROLL_BAR: Bar = Bar {
    size: ScrollBar::SIZE.index,
    position: ScrollBar::POSITION.index,
    active: ScrollBar::ACTIVE.index,
    orientation: ScrollBar::ORIENTATION.index,
    horizontal: ScrollBar::HORIZONTAL.index,
    vertical: ScrollBar::VERTICAL.index,
    minimum_size: ScrollBar::MINIMUM_SIZE.index,
    visual_size: ScrollBar::VISUAL_SIZE.index,
    visual_position: ScrollBar::VISUAL_POSITION.index,
    moves_content: true,
};

const INDICATOR: Bar = Bar {
    size: ScrollIndicator::SIZE.index,
    position: ScrollIndicator::POSITION.index,
    active: ScrollIndicator::ACTIVE.index,
    orientation: ScrollIndicator::ORIENTATION.index,
    horizontal: ScrollIndicator::HORIZONTAL.index,
    vertical: ScrollIndicator::VERTICAL.index,
    minimum_size: ScrollIndicator::MINIMUM_SIZE.index,
    visual_size: ScrollIndicator::VISUAL_SIZE.index,
    visual_position: ScrollIndicator::VISUAL_POSITION.index,
    moves_content: false,
};

fn bar_of(engine: &Engine, obj: ObjectId) -> Option<Bar> {
    if is_a::<ScrollBar>(engine, obj) {
        Some(SCROLL_BAR)
    } else if is_a::<ScrollIndicator>(engine, obj) {
        Some(INDICATOR)
    } else {
        None
    }
}

/// A write that is the template's and not the author's: a binding on the
/// property stays, as it does when Qt's own code sets one.
fn put(engine: &Engine, obj: ObjectId, prop: PropIndex, v: Value) {
    if !engine.is_alive(obj) || engine.untrack(|| engine.read(obj, prop)).change_equals(&v) {
        return;
    }
    check(engine, "scroll bar", engine.write_derived(obj, prop, v));
}

/// Where the handle starts and how long it is, as shares of the track.
///
/// A handle is never shown shorter than `minimum`; one that would be is
/// lengthened, and its travel shortened to match. Otherwise what shows is
/// the part of `[position, position + size]` that is on the track, so a
/// position past an end shortens the handle instead of moving it off.
pub(crate) fn visual(position: f64, size: f64, minimum: f64) -> (f64, f64) {
    let minimum = minimum.clamp(0.0, 1.0);
    if minimum > size {
        let room = 1.0 - size;
        let at = if room > 0.0 {
            position * (1.0 - minimum) / room
        } else {
            0.0
        };
        return (at.clamp(0.0, 1.0 - minimum), minimum);
    }
    let start = position.clamp(0.0, 1.0);
    let end = (position + size).clamp(0.0, 1.0);
    let long = (end - start).max(minimum);
    (start.min(1.0 - long), long)
}

fn is_horizontal(engine: &Engine, obj: ObjectId, bar: Bar) -> bool {
    num(engine, obj, bar.orientation) as i32 == orientation::HORIZONTAL
}

/// Where the content item of a bar or an indicator is: the handle, along
/// the control inside its padding (tracked). None for any other control.
pub(crate) fn handle_rect(engine: &Engine, obj: ObjectId) -> Option<[f64; 4]> {
    let bar = bar_of(engine, obj)?;
    let x = num(engine, obj, Control::LEFT_PADDING.index);
    let y = num(engine, obj, Control::TOP_PADDING.index);
    let w = num(engine, obj, Control::AVAILABLE_WIDTH.index);
    let h = num(engine, obj, Control::AVAILABLE_HEIGHT.index);
    let at = num(engine, obj, bar.visual_position);
    let long = num(engine, obj, bar.visual_size);
    Some(if is_horizontal(engine, obj, bar) {
        [x + at * w, y, long * w, h]
    } else {
        [x, y + at * h, w, long * h]
    })
}

fn init_bar(engine: &Engine, obj: ObjectId, bar: Bar, role: Role) -> Result<(), EngineError> {
    engine.bind_initial(obj, bar.horizontal, move |e| {
        Value::Bool(is_horizontal(e, obj, bar))
    })?;
    engine.bind_initial(obj, bar.vertical, move |e| {
        Value::Bool(num(e, obj, bar.orientation) as i32 == orientation::VERTICAL)
    })?;
    let shown = move |e: &Engine| {
        visual(
            num(e, obj, bar.position),
            num(e, obj, bar.size),
            num(e, obj, bar.minimum_size),
        )
    };
    engine.bind_initial(obj, bar.visual_position, move |e| Value::Real(shown(e).0))?;
    engine.bind_initial(obj, bar.visual_size, move |e| Value::Real(shown(e).1))?;

    // The size of a bar is a share: what is given outside of 0 to 1 is
    // brought in (an indicator keeps what it is given). A handle that a
    // new size would take past the end is moved back to end there.
    let bring_in = move |e: &Engine| {
        let given = e.untrack(|| num(e, obj, bar.size));
        let kept = if given.is_nan() {
            0.0
        } else if bar.moves_content {
            given.clamp(0.0, 1.0)
        } else {
            given
        };
        if kept != given {
            put(e, obj, bar.size, Value::Real(kept));
        }
        let position = e.untrack(|| num(e, obj, bar.position));
        if position + kept > 1.0 {
            put(e, obj, bar.position, Value::Real(1.0 - kept));
        }
    };
    bring_in(engine);
    engine.watch(obj, bar.size, move |e, _| bring_in(e))?;
    crate::a11y::defaults(engine, obj, role, None)
}

#[qml_impl]
impl ScrollBar {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        init_bar(e, obj, SCROLL_BAR, Role::ScrollBar)?;
        // A press does not outlive the bar's being usable.
        for p in [Item::ENABLED.index, Item::VISIBLE.index] {
            e.watch(obj, p, move |e, v| {
                if !v.to_boolean() {
                    cancel(e, obj);
                }
            })?;
        }
        Ok(())
    }

    /// `increase()`: a step towards the end.
    #[qml(invokable)]
    fn increase(cx: &Cx) {
        step(cx.engine, cx.this, true);
    }

    /// `decrease()`: a step towards the beginning.
    #[qml(invokable)]
    fn decrease(cx: &Cx) {
        step(cx.engine, cx.this, false);
    }
}

#[qml_impl]
impl ScrollIndicator {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_bar(cx.engine, cx.this, INDICATOR, Role::Indicator)
    }
}

/// How far the handle of a bar of this `size` can go.
fn travel(size: f64) -> f64 {
    (1.0 - size).max(0.0)
}

/// Moves a scroll bar by its step, as `increase()` and `decrease()` and
/// the arrow keys of a view do. The bar is active while it happens, so a
/// style that shows the handle only then shows it.
pub(crate) fn step(engine: &Engine, bar: ObjectId, forward: bool) {
    if !is_a::<ScrollBar>(engine, bar) {
        return;
    }
    let (position, size, step, active) = engine.untrack(|| {
        (
            num(engine, bar, ScrollBar::POSITION.index),
            num(engine, bar, ScrollBar::SIZE.index),
            num(engine, bar, ScrollBar::STEP_SIZE.index),
            flag(engine, bar, ScrollBar::ACTIVE.index),
        )
    });
    let step = if step > 0.0 { step } else { DEFAULT_STEP };
    let to = if forward {
        position + step
    } else {
        position - step
    };
    put(engine, bar, ScrollBar::ACTIVE.index, Value::Bool(true));
    put(
        engine,
        bar,
        ScrollBar::POSITION.index,
        Value::Real(to.clamp(0.0, travel(size))),
    );
    put(engine, bar, ScrollBar::ACTIVE.index, Value::Bool(active));
}

// ------------------------------------------------- assistive technology

/// A bar's node says where the handle is on its way from one end to the
/// other. An indicator's says the same and has no step: it takes nothing.
fn describe(engine: &Engine, obj: ObjectId, data: &mut NodeData) {
    let Some(bar) = bar_of(engine, obj) else {
        return;
    };
    let step = if bar.moves_content {
        Some(num(engine, obj, ScrollBar::STEP_SIZE.index))
            .filter(|s| *s > 0.0)
            .unwrap_or(DEFAULT_STEP)
    } else {
        0.0
    };
    data.range = Some(Range {
        value: num(engine, obj, bar.position),
        min: 0.0,
        max: travel(num(engine, obj, bar.size)),
        step,
    });
}

fn act(engine: &Engine, obj: ObjectId, action: AccessibleAction) -> bool {
    if !is_a::<ScrollBar>(engine, obj) {
        return false;
    }
    let before = engine.untrack(|| num(engine, obj, ScrollBar::POSITION.index));
    match action {
        AccessibleAction::Increase => step(engine, obj, true),
        AccessibleAction::Decrease => step(engine, obj, false),
        AccessibleAction::SetValue(to) if to.is_finite() => {
            let size = engine.untrack(|| num(engine, obj, ScrollBar::SIZE.index));
            let to = Value::Real(to.clamp(0.0, travel(size)));
            put(engine, obj, ScrollBar::POSITION.index, to);
        }
        _ => return false,
    }
    engine.untrack(|| num(engine, obj, ScrollBar::POSITION.index)) != before
}

// ----------------------------------------------------------- the pointer

fn held_state(engine: &Engine, obj: ObjectId) -> Option<Rc<Held>> {
    engine.with::<ScrollBar, _>(obj, |b| b.state.clone()).ok()
}

/// Where along the track the point is, as a share of it.
fn along(engine: &Engine, obj: ObjectId, at: kurbo::Point) -> f64 {
    engine.untrack(|| {
        let (lead, room, p) = if is_horizontal(engine, obj, SCROLL_BAR) {
            (
                Control::LEFT_PADDING.index,
                Control::AVAILABLE_WIDTH.index,
                at.x,
            )
        } else {
            (
                Control::TOP_PADDING.index,
                Control::AVAILABLE_HEIGHT.index,
                at.y,
            )
        };
        let room = num(engine, obj, room);
        if room > 0.0 {
            (p - num(engine, obj, lead)) / room
        } else {
            0.0
        }
    })
}

/// The nearest position a bar that snaps may have. The steps divide the
/// handle's travel, not the whole track (checked on qml6).
fn snapped(position: f64, step: f64, size: f64) -> f64 {
    let grid = step * travel(size);
    if grid <= 0.0 {
        return position;
    }
    ((position / grid).round() * grid).clamp(0.0, travel(size))
}

/// Puts the handle where the pointer has it.
fn drag_to(engine: &Engine, obj: ObjectId, st: &Held, at: kurbo::Point, snap: bool) {
    let (size, step) = engine.untrack(|| {
        (
            num(engine, obj, ScrollBar::SIZE.index),
            num(engine, obj, ScrollBar::STEP_SIZE.index),
        )
    });
    let mut to = (along(engine, obj, at) - st.offset.get()).clamp(0.0, travel(size));
    if snap {
        to = snapped(to, step, size);
    }
    put(engine, obj, ScrollBar::POSITION.index, Value::Real(to));
}

fn snaps(engine: &Engine, obj: ObjectId, mode: i32) -> bool {
    engine.untrack(|| num(engine, obj, ScrollBar::SNAP_MODE.index)) as i32 == mode
}

/// A bar shows while the pointer is over it or has it.
fn show_use(engine: &Engine, obj: ObjectId) {
    let used = engine.untrack(|| {
        flag(engine, obj, Control::HOVERED.index) || flag(engine, obj, ScrollBar::PRESSED.index)
    });
    put(engine, obj, ScrollBar::ACTIVE.index, Value::Bool(used));
}

fn press(engine: &Engine, obj: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    // A bar that takes no pointer is not in the way of what is under it:
    // the content of a Flickable can be dragged from there.
    if !takes_input(engine, obj)
        || !is_left(ev)
        || !engine.untrack(|| flag(engine, obj, ScrollBar::INTERACTIVE.index))
    {
        return false;
    }
    let Some(st) = held_state(engine, obj) else {
        return false;
    };
    let p = along(engine, obj, at);
    let (position, size, start, long) = engine.untrack(|| {
        (
            num(engine, obj, ScrollBar::POSITION.index),
            num(engine, obj, ScrollBar::SIZE.index),
            num(engine, obj, ScrollBar::VISUAL_POSITION.index),
            num(engine, obj, ScrollBar::VISUAL_SIZE.index),
        )
    });
    // On the handle the pointer keeps the spot it took; on the track the
    // middle of the handle comes to it.
    let on_handle = p >= start && p <= start + long;
    st.offset
        .set(if on_handle { p - position } else { size / 2.0 });
    st.held.set(true);
    // The Flickable the bar is on would take a press that moves this far
    // for a drag of its content.
    flickable::keep_pointer_grab(engine, obj, true);
    put(engine, obj, ScrollBar::PRESSED.index, Value::Bool(true));
    show_use(engine, obj);
    let snap = snaps(engine, obj, snap_mode::SNAP_ALWAYS);
    drag_to(engine, obj, &st, at, snap);
    true
}

fn moved(engine: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    if let Some(st) = held_state(engine, obj).filter(|st| st.held.get()) {
        let snap = snaps(engine, obj, snap_mode::SNAP_ALWAYS);
        drag_to(engine, obj, &st, at, snap);
    }
}

fn let_go(engine: &Engine, obj: ObjectId, st: &Held) {
    st.held.set(false);
    flickable::keep_pointer_grab(engine, obj, false);
    put(engine, obj, ScrollBar::PRESSED.index, Value::Bool(false));
    show_use(engine, obj);
}

fn release(engine: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    let Some(st) = held_state(engine, obj).filter(|st| st.held.get()) else {
        return;
    };
    let snap = !snaps(engine, obj, snap_mode::NO_SNAP);
    drag_to(engine, obj, &st, at, snap);
    let_go(engine, obj, &st);
}

fn cancel(engine: &Engine, obj: ObjectId) {
    if let Some(st) = held_state(engine, obj).filter(|st| st.held.get()) {
        let_go(engine, obj, &st);
    }
}

fn bar_hover(engine: &Engine, obj: ObjectId, over: bool) {
    hover(engine, obj, over);
    show_use(engine, obj);
}

fn pass(_: &Engine, _: ObjectId, _: &PointerEvent, _: kurbo::Point) -> bool {
    false
}

fn ignore(_: &Engine, _: ObjectId, _: &PointerEvent, _: kurbo::Point) {}

fn ignore_cancel(_: &Engine, _: ObjectId) {}

// ------------------------------------------------- attached to a Flickable

/// The Flickable a bar attached to `owner` is for: the owner, or the
/// content item of a view (tracked).
pub(crate) fn flickable_of(engine: &Engine, owner: ObjectId) -> Option<ObjectId> {
    if flickable::is_flickable(engine, owner) {
        return Some(owner);
    }
    if !is_a::<Control>(engine, owner) {
        return None;
    }
    object(engine, owner, Control::CONTENT_ITEM.index)
        .filter(|c| flickable::is_flickable(engine, *c))
}

/// One direction of a Flickable as a bar shows it: where the view starts
/// and how much it is, as shares of the whole, and the whole and the
/// margin before the content in pixels (tracked).
struct Share {
    position: f64,
    size: f64,
    total: f64,
    lead: f64,
}

fn share(engine: &Engine, f: ObjectId, horizontal: bool) -> Share {
    let axis = flickable::axis_state(engine, f, horizontal);
    let view = num(
        engine,
        f,
        if horizontal {
            Item::WIDTH.index
        } else {
            Item::HEIGHT.index
        },
    );
    // The margins count as content; content that fits is all in view.
    let total = (axis.max - axis.min + view).max(view);
    if total <= 0.0 {
        return Share {
            position: 0.0,
            size: 1.0,
            total: 0.0,
            lead: -axis.min,
        };
    }
    Share {
        position: (axis.position - axis.min) / total,
        size: view / total,
        total,
        lead: -axis.min,
    }
}

/// The bar in one slot of an attached object and what was put on it.
#[derive(Debug)]
struct Link {
    bar: ObjectId,
    layers: Vec<(PropIndex, OverrideId)>,
}

/// Makes the bar in `att`'s property `slot` the bar of `owner` in one
/// direction, now and whenever the property changes.
fn follow(
    engine: &Engine,
    owner: ObjectId,
    att: ObjectId,
    slot: PropIndex,
    horizontal: bool,
    kind: Bar,
) -> Result<(), EngineError> {
    let link: Rc<RefCell<Option<Link>>> = Rc::default();
    // Set while the bar is put where the content is. Putting it there can
    // lay the content out again (a view makes the rows that come into
    // sight), so the place it was given is already stale when the bar
    // hears it: the content never follows what it told the bar itself.
    let echo: Rc<Cell<bool>> = Rc::default();
    {
        let (link, echo) = (link.clone(), echo.clone());
        engine.watch(att, slot, move |e, v| {
            let bar = v.as_object().filter(|b| e.is_alive(*b));
            check(
                e,
                "scroll bar",
                adopt(e, owner, horizontal, kind, &link, &echo, bar),
            );
        })?;
    }
    let current = move |e: &Engine| {
        object(e, att, slot).filter(|b| e.is_alive(owner) && bar_of(e, *b).is_some())
    };

    // The bar follows the content.
    engine.effect_in(att, move |e| {
        let Some(bar) = current(e) else { return };
        let Some(f) = flickable_of(e, owner) else {
            return;
        };
        let s = share(e, f, horizontal);
        // In an order that never takes the handle past the end: a bar
        // brought back from there would move the content to a place it
        // is not at. A handle that grows moves first, one that shrinks
        // after.
        let was = echo.replace(true);
        e.untrack(|| {
            let now = num(e, bar, kind.position);
            // What the bar itself put the content at comes back as the
            // same position give or take the arithmetic.
            let moved = (now - s.position).abs() > 1e-12;
            let grows = s.size > num(e, bar, kind.size);
            if moved && grows {
                put(e, bar, kind.position, Value::Real(s.position));
            }
            put(e, bar, kind.size, Value::Real(s.size));
            if moved && !grows {
                put(e, bar, kind.position, Value::Real(s.position));
            }
        });
        echo.set(was);
    })?;

    // A bar is active while the content moves its way. Only a change says
    // so: a bar that was declared active stays so until the content has
    // moved and stopped.
    let was_moving = Cell::new(false);
    engine.effect_in(att, move |e| {
        let Some(bar) = current(e) else { return };
        let Some(f) = flickable_of(e, owner) else {
            return;
        };
        let moving = flag(
            e,
            f,
            if horizontal {
                Flickable::MOVING_HORIZONTALLY.index
            } else {
                Flickable::MOVING_VERTICALLY.index
            },
        );
        if was_moving.replace(moving) == moving {
            return;
        }
        e.untrack(|| {
            let used = is_a::<ScrollBar>(e, bar)
                && (flag(e, bar, Control::HOVERED.index) || flag(e, bar, ScrollBar::PRESSED.index));
            put(e, bar, kind.active, Value::Bool(moving || used));
        });
    })?;
    Ok(())
}

fn adopt(
    engine: &Engine,
    owner: ObjectId,
    horizontal: bool,
    kind: Bar,
    link: &Rc<RefCell<Option<Link>>>,
    echo: &Rc<Cell<bool>>,
    bar: Option<ObjectId>,
) -> Result<(), EngineError> {
    let old = link.borrow_mut().take();
    if let Some(old) = old {
        if Some(old.bar) == bar {
            *link.borrow_mut() = Some(old);
            return Ok(());
        }
        drop_layers(engine, old.bar, old.layers);
    }
    let Some(bar) = bar.filter(|b| is_item(engine, *b) && bar_of(engine, *b).is_some()) else {
        return Ok(());
    };
    let way = if horizontal {
        orientation::HORIZONTAL
    } else {
        orientation::VERTICAL
    };
    put(engine, bar, kind.orientation, Value::Int(way));
    let layers = place(engine, owner, bar, horizontal)?;
    *link.borrow_mut() = Some(Link { bar, layers });

    let is_current = {
        let link = link.clone();
        move || link.borrow().as_ref().is_some_and(|l| l.bar == bar)
    };
    {
        // A bar its author moves elsewhere is the author's to place.
        let (link, is_current) = (link.clone(), is_current.clone());
        engine.watch(bar, Item::PARENT.index, move |e, v| {
            if is_current() && v.as_object() != Some(owner) {
                let layers = link
                    .borrow_mut()
                    .as_mut()
                    .map(|l| std::mem::take(&mut l.layers));
                drop_layers(e, bar, layers.unwrap_or_default());
            }
        })?;
    }
    if kind.moves_content {
        // The content follows the bar.
        let echo = echo.clone();
        engine.watch(bar, kind.position, move |e, v| {
            if echo.get() || !is_current() || !e.is_alive(owner) {
                return;
            }
            let Some(f) = e.untrack(|| flickable_of(e, owner)) else {
                return;
            };
            let s = e.untrack(|| share(e, f, horizontal));
            if s.total <= 0.0 {
                return;
            }
            let to = v.to_number() * s.total - s.lead;
            // What the content put the bar at comes back as the same
            // place give or take the arithmetic.
            if (to - (s.position * s.total - s.lead)).abs() > 1e-9 {
                flickable::scroll(e, f, horizontal, to);
            }
        })?;
    }
    Ok(())
}

fn drop_layers(engine: &Engine, bar: ObjectId, layers: Vec<(PropIndex, OverrideId)>) {
    if !engine.is_alive(bar) {
        return;
    }
    for (prop, id) in layers {
        // Gone already if something assigned through the layer.
        let _ = engine.remove_override(bar, prop, id, RestoreMode::BindingOrValue);
    }
}

/// Makes the bar a child of `owner`, and along the edge of a Flickable:
/// the vertical one down the right (the left when mirrored), the
/// horizontal one along the bottom, each as long as the Flickable. A
/// position the author gave is kept. The bars of a view are placed by its
/// style, and a bar that was given another parent by its author.
fn place(
    engine: &Engine,
    owner: ObjectId,
    bar: ObjectId,
    horizontal: bool,
) -> Result<Vec<(PropIndex, OverrideId)>, EngineError> {
    let elsewhere = engine
        .untrack(|| engine.visual_parent(bar))
        .is_some_and(|p| p != owner);
    if elsewhere {
        return Ok(Vec::new());
    }
    // The content first, so that the bar is above it.
    flickable::content_item(engine, owner);
    set_parent_item(engine, bar, Some(owner))?;
    if !flickable::is_flickable(engine, owner) {
        return Ok(Vec::new());
    }
    let mut layers = Vec::new();
    let mut layer = |prop: PropIndex, f: Box<dyn FnMut(&Engine) -> Value>| {
        engine
            .push_override(bar, prop, OverrideContent::Binding(f))
            .map(|id| layers.push((prop, id)))
    };
    let unset = |p: PropIndex| is_unset(engine, bar, p);
    let (width, height) = (Item::WIDTH.index, Item::HEIGHT.index);
    if horizontal {
        if unset(Item::X.index) {
            layer(Item::X.index, Box::new(|_| Value::Real(0.0)))?;
        }
        if unset(Item::Y.index) {
            layer(
                Item::Y.index,
                Box::new(move |e| Value::Real(num(e, owner, height) - num(e, bar, height))),
            )?;
        }
        layer(width, Box::new(move |e| Value::Real(num(e, owner, width))))?;
    } else {
        if unset(Item::X.index) {
            layer(
                Item::X.index,
                Box::new(move |e| {
                    Value::Real(if is_mirrored(e, owner) {
                        0.0
                    } else {
                        num(e, owner, width) - num(e, bar, width)
                    })
                }),
            )?;
        }
        if unset(Item::Y.index) {
            layer(Item::Y.index, Box::new(|_| Value::Real(0.0)))?;
        }
        layer(
            height,
            Box::new(move |e| Value::Real(num(e, owner, height))),
        )?;
    }
    Ok(layers)
}

/// The bars attached to `owner`, horizontal then vertical, if any were
/// (tracked). Never makes the attached object.
pub(crate) fn scroll_bars(engine: &Engine, owner: ObjectId) -> [Option<ObjectId>; 2] {
    let attacher = MetaRef::Static(ScrollBar::static_meta());
    let Some(att) = engine.existing_attached_object(owner, &attacher) else {
        return [None, None];
    };
    [
        ScrollBarAttached::HORIZONTAL.index,
        ScrollBarAttached::VERTICAL.index,
    ]
    .map(|slot| object(engine, att, slot).filter(|b| is_a::<ScrollBar>(engine, *b)))
}

pub(crate) fn register(engine: &Engine) {
    register_pointer_item(
        engine,
        MetaRef::Static(ScrollBar::static_meta()),
        PointerItem {
            press,
            moved,
            release,
            cancel,
            hover: bar_hover,
        },
    );
    for meta in [ScrollBar::static_meta(), ScrollIndicator::static_meta()] {
        register_accessible_item(
            engine,
            MetaRef::Static(meta),
            AccessibleItem { describe, act },
        );
    }
    // An indicator is not in the way of the pointer.
    register_pointer_item(
        engine,
        MetaRef::Static(ScrollIndicator::static_meta()),
        PointerItem {
            press: pass,
            moved: ignore,
            release: ignore,
            cancel: ignore_cancel,
            hover,
        },
    );
    engine.on_attached_created(|e, owner, att| {
        let slots = if is_a::<ScrollBarAttached>(e, att) {
            Some((
                SCROLL_BAR,
                ScrollBarAttached::HORIZONTAL.index,
                ScrollBarAttached::VERTICAL.index,
            ))
        } else if is_a::<ScrollIndicatorAttached>(e, att) {
            Some((
                INDICATOR,
                ScrollIndicatorAttached::HORIZONTAL.index,
                ScrollIndicatorAttached::VERTICAL.index,
            ))
        } else {
            None
        };
        if let Some((kind, across, down)) = slots {
            for (slot, horizontal) in [(across, true), (down, false)] {
                let r = follow(e, owner, att, slot, horizontal, kind);
                check(e, "scroll bar", r);
            }
        }
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn what_shows_is_the_part_of_the_handle_on_the_track() {
        assert!(close(visual(0.0, 0.25, 0.0), (0.0, 0.25)));
        assert!(close(visual(0.5, 0.25, 0.0), (0.5, 0.25)));
        assert!(close(visual(0.9, 0.25, 0.0), (0.9, 0.1)));
        assert!(close(visual(-0.1, 0.25, 0.0), (0.0, 0.15)));
        // Never shorter than the minimum, and still on the track.
        assert!(close(visual(0.99, 0.25, 0.05), (0.95, 0.05)));
    }

    #[test]
    fn a_handle_at_its_minimum_travels_the_rest_of_the_track() {
        assert!(close(visual(0.0, 0.2, 0.4), (0.0, 0.4)));
        assert!(close(visual(0.4, 0.2, 0.4), (0.3, 0.4)));
        assert!(close(visual(0.8, 0.2, 0.4), (0.6, 0.4)));
        assert!(close(visual(2.0, 0.2, 0.4), (0.6, 0.4)));
        assert!(close(visual(0.0, 1.0, 2.0), (0.0, 1.0)));
    }

    #[test]
    fn steps_divide_the_travel_of_the_handle() {
        assert!((snapped(0.426, 0.2, 0.25) - 0.45).abs() < 1e-9);
        assert!((snapped(0.2015, 0.2, 0.25) - 0.15).abs() < 1e-9);
        assert!((snapped(0.74, 0.2, 0.25) - 0.75).abs() < 1e-9);
        assert_eq!(snapped(0.426, 0.0, 0.25), 0.426);
        assert_eq!(snapped(0.3, 0.2, 1.0), 0.3);
    }
}
